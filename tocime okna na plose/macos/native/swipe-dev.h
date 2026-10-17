// macOS 26 Dock gesture ABI. Field meanings follow InstantSpaceSwitcher (MIT).
// https://github.com/jurplel/InstantSpaceSwitcher/blob/main/Sources/ISS/ISS.c
// Attribution and license are in THIRD-PARTY-NOTICES.txt.
// PopSpace offers a moderate fling for animation and a high-speed preset.
static BOOL postSwipe(int direction,int phase,double progress,double velocity,CGPoint location) {
    CGEventRef event=CGEventCreate(NULL);
    if (!event) return NO;
    CGEventSetFlags(event,0);
    CGEventSetLocation(event,location);
    CGEventSetIntegerValueField(event,(CGEventField)55,30);
    CGEventSetIntegerValueField(event,(CGEventField)110,23);
    CGEventSetIntegerValueField(event,(CGEventField)123,1);
    CGEventSetIntegerValueField(event,(CGEventField)132,phase);
    CGEventSetDoubleValueField(event,(CGEventField)124,direction*progress);
    CGEventSetDoubleValueField(event,(CGEventField)129,direction*velocity);
    // The macOS 26 Dock payload carries the signed velocity in both fields,
    // despite horizontal motion. Match the working ISS event format exactly.
    CGEventSetDoubleValueField(event,(CGEventField)130,direction*velocity);
    CGEventPost(kCGSessionEventTap,event);
    CFRelease(event);
    return YES;
}
static BOOL animateSwipe(int direction,double velocity,CGPoint location,void (^complete)(void)) {
    // macOS 27 changes the event format. Fail closed there instead of sending
    // a partially recognized gesture which could wedge Mission Control.
    if (NSProcessInfo.processInfo.operatingSystemVersion.majorVersion!=26) return NO;
    // Keep the gesture atomic; the Dock owns the animation. Do not stream
    // cumulative progress values: that can overshoot or fight its own motion.
    for (int i=0;i<3;i++) {
        int phases[]={1,2,4};
        if (!postSwipe(direction,phases[i],0.000016,velocity,location)) {
            postSwipe(direction,8,0,0,location); return NO;
        }
    }
    complete();
    return YES;
}
