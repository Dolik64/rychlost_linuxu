#import <Foundation/Foundation.h>
#import <ApplicationServices/ApplicationServices.h>
#import <objc/runtime.h>
#import <dlfcn.h>
#import <mach-o/dyld.h>
#import <mach-o/nlist.h>

static void *localSymbol(const char *name) {
    for (uint32_t i=0; i<_dyld_image_count(); i++) {
        if (!strstr(_dyld_get_image_name(i), "/SkyLight.framework/")) continue;
        const struct mach_header_64 *header=(const void *)_dyld_get_image_header(i);
        if (header->magic!=MH_MAGIC_64) continue;
        const struct load_command *command=(const void *)(header+1);
        const struct symtab_command *symbols=NULL;
        const struct segment_command_64 *linkedit=NULL;
        for (uint32_t c=0; c<header->ncmds; c++) {
            if (command->cmd==LC_SYMTAB) symbols=(const void *)command;
            if (command->cmd==LC_SEGMENT_64 && !strcmp(((const struct segment_command_64 *)command)->segname,"__LINKEDIT")) linkedit=(const void *)command;
            command=(const void *)((const char *)command+command->cmdsize);
        }
        if (!symbols || !linkedit) continue;
        intptr_t slide=_dyld_get_image_vmaddr_slide(i);
        uintptr_t base=slide+linkedit->vmaddr-linkedit->fileoff;
        const struct nlist_64 *entries=(const void *)(base+symbols->symoff);
        const char *strings=(const char *)(base+symbols->stroff);
        for (uint32_t n=0; n<symbols->nsyms; n++) {
            if (entries[n].n_un.n_strx>=symbols->strsize) continue;
            if (!strcmp(strings+entries[n].n_un.n_strx,name)) return (void *)(slide+entries[n].n_value);
        }
    }
    return NULL;
}
int main(void) { @autoreleasepool {
    dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight",RTLD_NOW|RTLD_GLOBAL);
    const char *name="__ZL54SLSPerformAsynchronousBridgedWindowManagementOperationP47SLSAsynchronousBridgedWindowManagementOperation";
    printf("OS: %s\n",NSProcessInfo.processInfo.operatingSystemVersionString.UTF8String);
    printf("Accessibility: %s\n",AXIsProcessTrusted()?"granted":"not-granted");
    printf("Bridge function present: %s\n",localSymbol(name)?"yes":"no");
    for (NSString *className in @[@"SLSBridgedMoveWindowsToManagedSpaceOperation",@"SLSAsynchronousBridgedWindowManagementOperation",@"SLSBridgedManagedDisplaySetCurrentSpaceOperation",@"SLSBridgedWillSwitchSpacesOperation"]) {
        Class cls=NSClassFromString(className);
        printf("Class %s: %s\n",className.UTF8String,cls?"present":"absent");
        unsigned int count=0;
        Method *methods=class_copyMethodList(cls,&count);
        for (unsigned int i=0;i<count;i++) printf("  %s %s\n",sel_getName(method_getName(methods[i])),method_getTypeEncoding(methods[i]));
        free(methods);
    }
    unsigned int allCount=0; Class *all=objc_copyClassList(&allCount);
    for (unsigned int i=0;i<allCount;i++) {
        const char *n=class_getName(all[i]);
        if (strstr(n,"SLSBridged") && (strstr(n,"Space") || strstr(n,"Focus"))) printf("Related: %s\n",n);
    }
    free(all);
    return 0;
}}
