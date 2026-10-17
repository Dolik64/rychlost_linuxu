#import <Cocoa/Cocoa.h>
#import <Carbon/Carbon.h>
#import <ApplicationServices/ApplicationServices.h>
#import <objc/runtime.h>
#import <objc/message.h>
#import <dlfcn.h>
#import "symbols.h"
#import "swipe.h"

// Private macOS ABI: resolved at runtime and checked before use.
// Bridge entry-point discovery: https://github.com/asmvik/yabai/issues/2788
static int (*connectionID)(void);
static CFArrayRef (*copyDisplays)(int);
static CFArrayRef (*copyWindowSpaces)(int,int,CFArrayRef);
static uint64_t (*currentSpace)(int,CFStringRef);
static bool (*isAnimating)(int,CFStringRef);
static AXError (*windowID)(AXUIElementRef,uint32_t *);
static int64_t (*performOperation)(void *);
static Class moveClass;
// Typed init declarations give ARC the correct ownership for private classes.
@protocol SpaceOperation
- (instancetype)initWithWindows:(NSArray *)windows spaceID:(uint64_t)space;
@end
static BOOL ready, busy, registered;
static EventHotKeyRef hotkeys[4];
static EventHandlerRef keyHandler;
static NSTextField *statusText;
static NSWindow *panel;
static NSStatusItem *statusItem;
static NSButton *enableButton;
static NSPopUpButton *speedButton;
static void (^testCompletion)(BOOL);
static BOOL testing;
static BOOL fromPhysicalHotkey;
static BOOL heldModifiers(void) {
    CGEventFlags flags=CGEventSourceFlagsState(kCGEventSourceStateHIDSystemState);
    CGEventFlags mask=kCGEventFlagMaskAlternate|kCGEventFlagMaskControl|kCGEventFlagMaskShift;
    return (flags&mask)==mask;
}
static double clockNow(void) { return NSProcessInfo.processInfo.systemUptime; }
static void report(NSString *message) {
    statusText.stringValue=message;
    fprintf(stdout,"%s\n",message.UTF8String); fflush(stdout);
}
static id attribute(AXUIElementRef element,CFStringRef name) {
    CFTypeRef result=NULL;
    if (!element || AXUIElementCopyAttributeValue(element,name,&result)!=kAXErrorSuccess) return nil;
    return CFBridgingRelease(result);
}
static NSArray *spacesForWindow(uint32_t wid) {
    return CFBridgingRelease(copyWindowSpaces(connectionID(),7,(__bridge CFArrayRef)@[@(wid)])) ?: @[];
}
static NSDictionary *displayForSpace(uint64_t sid) {
    NSArray *displays=CFBridgingRelease(copyDisplays(connectionID()));
    for (NSDictionary *display in displays) {
        for (NSDictionary *space in display[@"Spaces"]) {
            if ([space[@"id64"] unsignedLongLongValue]==sid) return display;
        }
    }
    return nil;
}
static BOOL initializeAPI(void) {
    void *sky=dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight",RTLD_NOW|RTLD_GLOBAL);
    if (!sky) return NO;
    connectionID=dlsym(sky,"SLSMainConnectionID");
    copyDisplays=dlsym(sky,"SLSCopyManagedDisplaySpaces");
    copyWindowSpaces=dlsym(sky,"SLSCopySpacesForWindows");
    currentSpace=dlsym(sky,"SLSManagedDisplayGetCurrentSpace");
    isAnimating=dlsym(sky,"SLSManagedDisplayIsAnimating");
    windowID=dlsym(RTLD_DEFAULT,"_AXUIElementGetWindow");
    performOperation=localSymbol("__ZL54SLSPerformAsynchronousBridgedWindowManagementOperationP47SLSAsynchronousBridgedWindowManagementOperation");
    moveClass=NSClassFromString(@"SLSBridgedMoveWindowsToManagedSpaceOperation");
    return connectionID && copyDisplays && copyWindowSpaces && currentSpace && windowID && performOperation
        && [moveClass instancesRespondToSelector:NSSelectorFromString(@"initWithWindows:spaceID:")];
}
static void moveDirect(uint32_t wid,uint64_t target) {
    // Never invoke the older API or invokeFallback: those can silently do nothing.
    id operation=[(id<SpaceOperation>)[moveClass alloc] initWithWindows:@[@(wid)] spaceID:target];
    performOperation((__bridge void *)operation);
}
static void focus(AXUIElementRef win,pid_t pid) {
    AXUIElementSetAttributeValue(win,kAXMainAttribute,kCFBooleanTrue);
    // NSRunningApplication activation can leave Finder's desktop (or another
    // application's old window) active after a Space transition. Bring forward
    // only the main window we just selected, as Hammerspoon's focus path does.
    ProcessSerialNumber psn;
    if (GetProcessForPID(pid,&psn)==noErr)
        SetFrontProcessWithOptions(&psn,kSetFrontProcessFrontWindowOnly);
    AXUIElementPerformAction(win,kAXRaiseAction);
    // Finder may focus the desktop during application activation. Reassert the
    // exact main window after activation; the job also verifies the real focus.
    AXUIElementSetAttributeValue(win,kAXMainAttribute,kCFBooleanTrue);
}
static id focusedWindow(pid_t *pid) {
    AXUIElementRef system=AXUIElementCreateSystemWide();
    id app=attribute(system,kAXFocusedApplicationAttribute); CFRelease(system);
    if (!app) return nil;
    AXUIElementGetPid((__bridge AXUIElementRef)app,pid);
    return attribute((__bridge AXUIElementRef)app,kAXFocusedWindowAttribute);
}
static BOOL focused(AXUIElementRef win,pid_t pid) {
    pid_t actualPID=0;
    id actual=focusedWindow(&actualPID);
    uint32_t expectedID=0,actualID=0;
    if (!actual || actualPID!=pid) return NO;
    windowID(win,&expectedID);
    windowID((__bridge AXUIElementRef)actual,&actualID);
    return expectedID && expectedID==actualID;
}
static BOOL sameFrame(AXUIElementRef win,CGPoint position,CGSize size) {
    id p=attribute(win,kAXPositionAttribute),s=attribute(win,kAXSizeAttribute);
    CGPoint actualP; CGSize actualS;
    return p && s && AXValueGetValue((__bridge AXValueRef)p,kAXValueCGPointType,&actualP)
        && AXValueGetValue((__bridge AXValueRef)s,kAXValueCGSizeType,&actualS)
        && fabs(actualP.x-position.x)<3 && fabs(actualP.y-position.y)<3
        && fabs(actualS.width-size.width)<3 && fabs(actualS.height-size.height)<3;
}
static void finish(BOOL success,NSString *message) {
    busy=NO; report(message);
    void (^completion)(BOOL)=testCompletion; testCompletion=nil;
    if (completion) completion(success);
}
static BOOL onScreen(uint32_t wid) {
    NSArray *windows=CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly,kCGNullWindowID));
    for (NSDictionary *info in windows) if ([info[(__bridge NSString *)kCGWindowNumber] unsignedIntValue]==wid) return YES;
    return NO;
}
static void moveWindow(AXUIElementRef input,int direction) {
    if (busy || !ready) return;
    if (!AXIsProcessTrusted()) { finish(NO,@"PopSpace potřebuje povolit Zpřístupnění."); return; }
    id role=attribute(input,kAXSubroleAttribute);
    if (![role isEqual:@"AXStandardWindow"] || [attribute(input,CFSTR("AXFullScreen")) boolValue]) {
        finish(NO,@"Vyber běžné okno mimo celou obrazovku."); return;
    }
    uint32_t wid=0; pid_t pid=0;
    if (windowID(input,&wid)!=kAXErrorSuccess || !wid) { finish(NO,@"Okno nelze určit."); return; }
    AXUIElementGetPid(input,&pid);
    NSArray *sourceSpaces=spacesForWindow(wid);
    if (sourceSpaces.count!=1) { finish(NO,@"Okno musí patřit jedné běžné ploše."); return; }
    uint64_t source=[sourceSpaces.firstObject unsignedLongLongValue];
    NSDictionary *display=displayForSpace(source);
    NSMutableArray *users=[NSMutableArray array];
    for (NSDictionary *space in display[@"Spaces"]) if ([space[@"type"] intValue]==0) [users addObject:space[@"id64"]];
    NSUInteger index=[users indexOfObject:@(source)];
    NSInteger next=(NSInteger)index+direction;
    if (index==NSNotFound || next<0 || next>=(NSInteger)users.count) { finish(NO,@"V tomto směru není další běžná plocha."); return; }
    uint64_t target=[users[next] unsignedLongLongValue];
    NSString *displayID=display[@"Display Identifier"];
    if (!displayID) { finish(NO,@"Monitor cílové plochy není dostupný."); return; }
    id p=attribute(input,kAXPositionAttribute),s=attribute(input,kAXSizeAttribute);
    CGPoint position; CGSize size;
    if (!p || !s || !AXValueGetValue((__bridge AXValueRef)p,kAXValueCGPointType,&position)
        || !AXValueGetValue((__bridge AXValueRef)s,kAXValueCGSizeType,&size)) { finish(NO,@"Rozměry okna nejsou dostupné."); return; }
    id heldWindow=CFBridgingRelease(CFRetain(input));
    // A gesture can only reach the immediate physical neighbour. Across a
    // fullscreen Space use the verified normal activation path instead.
    NSArray *raw=display[@"Spaces"];
    NSInteger rawSource=-1,rawTarget=-1;
    for (NSUInteger i=0;i<raw.count;i++) {
        if ([raw[i][@"id64"] unsignedLongLongValue]==source) rawSource=i;
        if ([raw[i][@"id64"] unsignedLongLongValue]==target) rawTarget=i;
    }
    BOOL fast=abs((int)(rawTarget-rawSource))==1 && speedButton.indexOfSelectedItem!=2;
    // Gesture dispatch is reliable on the display under the pointer. On other
    // monitors keep the normal path; never move the user's pointer.
    CGEventRef sample=CGEventCreate(NULL);
    CGPoint pointer=sample?CGEventGetLocation(sample):CGPointZero;
    if (sample) CFRelease(sample);
    CGDirectDisplayID pointerDisplay=0; uint32_t count=0;
    CGGetDisplaysWithPoint(pointer,1,&pointerDisplay,&count);
    CFUUIDRef uuid=count?CGDisplayCreateUUIDFromDisplayID(pointerDisplay):NULL;
    NSString *pointerDisplayID=uuid?CFBridgingRelease(CFUUIDCreateString(NULL,uuid)):nil;
    if (uuid) CFRelease(uuid);
    fast=fast && [pointerDisplayID isEqual:displayID];
    double velocity=speedButton.indexOfSelectedItem==1?2000:15;
    BOOL physical=fromPhysicalHotkey;
    busy=YES;
    report([NSString stringWithFormat:@"Test okna %u: plocha %llu → %llu.",wid,source,target]);
    double started=clockNow();
    __block BOOL submitted=NO,activated=NO,gesture=NO,gestureEnded=NO;
    __block double submittedAt=0,gestureAt=0,stableAt=0,lastFocusAt=0;
    NSTimer *moveTimer=[NSTimer timerWithTimeInterval:0.01 repeats:YES block:^(NSTimer *timer) {
        AXUIElementRef win=(__bridge AXUIElementRef)heldWindow;
        if (clockNow()-started>5) {
            [timer invalidate];
            pid_t actualPID=0; uint32_t actualID=0;
            id actualWindow=focusedWindow(&actualPID);
            if (actualWindow) windowID((__bridge AXUIElementRef)actualWindow,&actualID);
            finish(NO,[NSString stringWithFormat:@"Nedokončeno: cílová plocha %llu, aktivní %llu; fokus %u/%d místo %u/%d; animace %@.",target,currentSpace(connectionID(),(__bridge CFStringRef)displayID),actualID,actualPID,wid,pid,(isAnimating && isAnimating(connectionID(),(__bridge CFStringRef)displayID))?@"běží":@"skončila"]); return;
        }
        if (!submitted) {
            // Native movement does not synthesize keys or mouse input, so it can
            // run while the user still holds the shortcut modifiers.
            moveDirect(wid,target); submitted=YES; submittedAt=clockNow(); return;
        }
        NSArray *actual=spacesForWindow(wid);
        if (actual.count!=1 || [actual.firstObject unsignedLongLongValue]!=target) {
            if (clockNow()-submittedAt>1.5) {
                [timer invalidate]; finish(NO,@"Přímé systémové volání okno nepřesunulo.");
            }
            return;
        }
        uint64_t active=currentSpace(connectionID(),(__bridge CFStringRef)displayID);
        if (!activated && !gesture && fast && active==source) {
            gesture=animateSwipe(direction,velocity,pointer,^{ gestureEnded=YES; });
            if (gesture) { gestureAt=clockNow(); return; }
        }
        if (gesture && !gestureEnded) return;
        if (gesture && active!=target && clockNow()-gestureAt<0.8) return;
        BOOL moving=isAnimating && isAnimating(connectionID(),(__bridge CFStringRef)displayID);
        if (gesture && moving) return;
        if (!activated || (!moving && !focused(win,pid) && clockNow()-lastFocusAt>0.15)) {
            focus(win,pid); activated=YES; lastFocusAt=clockNow(); return;
        }
        if (active==target && !moving && focused(win,pid) && onScreen(wid)) {
            if (!stableAt) { stableAt=clockNow(); return; }
            if (clockNow()-stableAt<0.08) return;
            [timer invalidate];
            BOOL geometry=sameFrame(win,position,size);
            finish(geometry,geometry ? [NSString stringWithFormat:@"Přesun potvrzen: %.2f s (%@). Plocha, fokus i rozměry souhlasí.%@",clockNow()-submittedAt,gesture?@"rychlý přechod":@"animace macOS",physical&&heldModifiers()?@" Dokončeno při stále drženém Option + Control + Shift.":@""]
                : @"Okno se přesunulo, ale změnily se jeho rozměry.");
        } else stableAt=0;
    }];
    [NSRunLoop.mainRunLoop addTimer:moveTimer forMode:NSRunLoopCommonModes];
}
static void moveFocused(int direction) {
    if (testing) return;
    pid_t pid=0;
    id win=focusedWindow(&pid);
    if (win) moveWindow((__bridge AXUIElementRef)win,direction);
    else report(@"Aktivní okno není dostupné.");
}
static OSStatus hotkeyHandler(EventHandlerCallRef next,EventRef event,void *context) {
    (void)next; (void)context;
    EventHotKeyID key;
    GetEventParameter(event,kEventParamDirectObject,typeEventHotKeyID,NULL,sizeof(key),NULL,&key);
    fromPhysicalHotkey=YES;
    moveFocused(key.id<2?-1:1);
    fromPhysicalHotkey=NO; return noErr;
}
@interface Delegate:NSObject<NSApplicationDelegate>
@end
@implementation Delegate
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;
    ready=initializeAPI();
    panel=[[NSWindow alloc] initWithContentRect:NSMakeRect(0,0,660,360)
        styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable backing:NSBackingStoreBuffered defer:NO];
    panel.title=@"PopSpace — nativní přesun oken";
    panel.releasedWhenClosed=NO;
    NSTextField *heading=[NSTextField labelWithString:@"Přesun okna přímo přes macOS"];
    heading.font=[NSFont boldSystemFontOfSize:21]; heading.frame=NSMakeRect(24,303,610,30); [panel.contentView addSubview:heading];
    NSTextField *description=[NSTextField wrappingLabelWithString:@"Option + Control + Shift + šipka přesune aktivní okno a přejde s ním.\n← / ↑ předchozí plocha · → / ↓ následující plocha\nZkratka funguje i při držených klávesách."];
    description.frame=NSMakeRect(24,218,610,72); [panel.contentView addSubview:description];
    NSTextField *speedLabel=[NSTextField labelWithString:@"Přechod ploch:"];
    speedLabel.frame=NSMakeRect(24,179,120,26); [panel.contentView addSubview:speedLabel];
    speedButton=[[NSPopUpButton alloc] initWithFrame:NSMakeRect(150,178,300,30) pullsDown:NO];
    [speedButton addItemsWithTitles:@[@"Rychlá animace",@"Téměř okamžitě",@"Běžná animace macOS"]];
    NSInteger saved=[NSUserDefaults.standardUserDefaults integerForKey:@"transition"];
    [speedButton selectItemAtIndex:MAX(0,MIN(2,saved))];
    speedButton.target=self; speedButton.action=@selector(speed:); [panel.contentView addSubview:speedButton];
    statusText=[NSTextField wrappingLabelWithString:ready ? @"Přímé systémové rozhraní je dostupné. Ovládání zatím není zapnuté." : @"Tato verze macOS nemá potřebné rozhraní."];
    statusText.frame=NSMakeRect(24,88,610,77); [panel.contentView addSubview:statusText];
    NSArray *titles=@[@"Povolit Zpřístupnění",@"Zapnout zkratky",@"Test tam a zpět"];
    SEL actions[]={@selector(permission:),@selector(enable:),@selector(test:)};
    for (int i=0;i<3;i++) {
        NSButton *button=[NSButton buttonWithTitle:titles[i] target:self action:actions[i]];
        button.frame=NSMakeRect(24+i*204,28,194,36); [panel.contentView addSubview:button];
        if (i==1) enableButton=button;
    }
    NSMenu *menu=[NSMenu new]; NSMenuItem *item=[NSMenuItem new]; [menu addItem:item];
    NSMenu *appMenu=[NSMenu new]; [appMenu addItemWithTitle:@"Ukončit PopSpace" action:@selector(terminate:) keyEquivalent:@"q"];
    item.submenu=appMenu; NSApp.mainMenu=menu;
    statusItem=[NSStatusBar.systemStatusBar statusItemWithLength:NSVariableStatusItemLength];
    statusItem.button.title=@"↔"; statusItem.button.toolTip=@"PopSpace";
    NSMenu *statusMenu=[NSMenu new];
    NSMenuItem *show=[statusMenu addItemWithTitle:@"Otevřít PopSpace" action:@selector(show:) keyEquivalent:@""]; show.target=self;
    [statusMenu addItem:[NSMenuItem separatorItem]];
    [statusMenu addItemWithTitle:@"Ukončit PopSpace" action:@selector(terminate:) keyEquivalent:@""];
    statusItem.menu=statusMenu;
    [panel center]; [panel makeKeyAndOrderFront:nil]; [NSApp activateIgnoringOtherApps:YES];
    if ([NSUserDefaults.standardUserDefaults boolForKey:@"hotkeysEnabled"] && AXIsProcessTrusted()) [self enable:nil];
}
- (void)show:(id)sender { (void)sender; [panel makeKeyAndOrderFront:nil]; [NSApp activateIgnoringOtherApps:YES]; }
- (void)speed:(id)sender { (void)sender; [NSUserDefaults.standardUserDefaults setInteger:speedButton.indexOfSelectedItem forKey:@"transition"]; }
- (void)permission:(id)sender {
    (void)sender;
    NSDictionary *options=@{(__bridge NSString *)kAXTrustedCheckOptionPrompt:@YES};
    BOOL granted=AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)options);
    report(granted ? @"Zpřístupnění je povoleno." : @"V nastavení macOS povol PopSpace v oddílu Zpřístupnění.");
}
- (void)enable:(id)sender {
    (void)sender;
    if (!ready || !AXIsProcessTrusted()) { report(@"Nejprve je potřeba Zpřístupnění pro PopSpace."); return; }
    if (registered) {
        for (int i=0;i<4;i++) { UnregisterEventHotKey(hotkeys[i]); hotkeys[i]=NULL; }
        registered=NO; enableButton.title=@"Zapnout zkratky";
        [NSUserDefaults.standardUserDefaults setBool:NO forKey:@"hotkeysEnabled"];
        report(@"Zkratky jsou vypnuté."); return;
    }
    UInt32 keys[]={kVK_LeftArrow,kVK_UpArrow,kVK_RightArrow,kVK_DownArrow};
    EventTypeSpec type={kEventClassKeyboard,kEventHotKeyPressed};
    if (!keyHandler) InstallApplicationEventHandler(hotkeyHandler,1,&type,NULL,&keyHandler);
    for (int i=0;i<4;i++) {
        EventHotKeyID key={.signature='PSpc',.id=(UInt32)i};
        OSStatus result=RegisterEventHotKey(keys[i],optionKey|controlKey|shiftKey,key,GetApplicationEventTarget(),kEventHotKeyExclusive,&hotkeys[i]);
        if (result!=noErr) {
            for (int j=0;j<i;j++) { UnregisterEventHotKey(hotkeys[j]); hotkeys[j]=NULL; }
            report(@"Zkratku používá jiný program. Původní vazby je potřeba vypnout."); return;
        }
    }
    registered=YES; enableButton.title=@"Vypnout zkratky";
    [NSUserDefaults.standardUserDefaults setBool:YES forKey:@"hotkeysEnabled"];
    report(@"Zkratky jsou zapnuté. Okno nastavení můžeš zavřít; PopSpace zůstane v horní liště.");
}
- (void)test:(id)sender {
    (void)sender;
    if (busy || testing) return;
    if (!AXIsProcessTrusted()) { report(@"Test cizího okna vyžaduje Zpřístupnění."); return; }
    for (NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
        if (![app.bundleIdentifier isEqual:@"com.apple.finder"] && ![app.bundleIdentifier isEqual:@"com.apple.TextEdit"]) continue;
        AXUIElementRef element=AXUIElementCreateApplication(app.processIdentifier);
        NSArray *windows=attribute(element,kAXWindowsAttribute); CFRelease(element);
        for (id value in windows) {
            AXUIElementRef win=(__bridge AXUIElementRef)value;
            NSString *title=attribute(win,kAXTitleAttribute);
            if (![title hasPrefix:@"Codex-"]) continue;
            NSArray *all; uint32_t wid=0; windowID(win,&wid); all=spacesForWindow(wid);
            uint64_t sid=[all.firstObject unsignedLongLongValue];
            NSMutableArray *users=[NSMutableArray array];
            for (NSDictionary *space in displayForSpace(sid)[@"Spaces"]) if ([space[@"type"] intValue]==0) [users addObject:space[@"id64"]];
            NSUInteger index=[users indexOfObject:@(sid)];
            int direction=(index!=NSNotFound && index+1<users.count)?1:-1;
            if (index==NSNotFound || users.count<2) { report(@"Test vyžaduje dvě běžné plochy."); return; }
            id p=attribute(win,kAXPositionAttribute),s=attribute(win,kAXSizeAttribute);
            CGPoint originalP; CGSize originalS;
            if (!p || !s || !AXValueGetValue((__bridge AXValueRef)p,kAXValueCGPointType,&originalP)
                || !AXValueGetValue((__bridge AXValueRef)s,kAXValueCGSizeType,&originalS)) continue;
            id held=value;
            testing=YES;
            NSMutableArray *results=[NSMutableArray array];
            __block int step=0;
            __block void (^run)(void);
            run=^{
                testCompletion=^(BOOL success) {
                    [results addObject:statusText.stringValue]; step++;
                    if (!success || step==4) {
                        AXValueRef pos=AXValueCreate(kAXValueCGPointType,&originalP),sz=AXValueCreate(kAXValueCGSizeType,&originalS);
                        AXUIElementSetAttributeValue((__bridge AXUIElementRef)held,kAXSizeAttribute,sz);
                        AXUIElementSetAttributeValue((__bridge AXUIElementRef)held,kAXPositionAttribute,pos);
                        CFRelease(pos); CFRelease(sz);
                        testing=NO; run=nil;
                        report([NSString stringWithFormat:@"Test %@ (%d/4): %@",success?@"prošel":@"selhal",step,[results componentsJoinedByString:@" | "]]); return;
                    }
                    if (step==2) {
                        // Maximise only our explicitly named scratch window.
                        NSRect frame=NSScreen.mainScreen.visibleFrame;
                        CGPoint pos=CGPointMake(frame.origin.x,NSScreen.screens.firstObject.frame.size.height-NSMaxY(frame));
                        CGSize size=frame.size;
                        AXValueRef pv=AXValueCreate(kAXValueCGPointType,&pos),sv=AXValueCreate(kAXValueCGSizeType,&size);
                        AXUIElementSetAttributeValue((__bridge AXUIElementRef)held,kAXPositionAttribute,pv);
                        AXUIElementSetAttributeValue((__bridge AXUIElementRef)held,kAXSizeAttribute,sv);
                        CFRelease(pv); CFRelease(sv);
                    }
                    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,200*NSEC_PER_MSEC),dispatch_get_main_queue(),^{ if (run) run(); });
                };
                moveWindow((__bridge AXUIElementRef)held,step%2?-direction:direction);
            };
            run(); return;
        }
    }
    report(@"Otevři testovací okno s názvem začínajícím Codex- ve Finderu nebo TextEditu.");
}
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender { (void)sender; return !registered; }
@end
int main(int argc,const char **argv) { @autoreleasepool {
    if (argc>1 && !strcmp(argv[1],"--probe")) {
        printf("API: %s\nAccessibility: %s\n",initializeAPI()?"available":"unavailable",AXIsProcessTrusted()?"granted":"not-granted"); return 0;
    }
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
    Delegate *delegate=[Delegate new]; NSApp.delegate=delegate; [NSApp run];
}}
