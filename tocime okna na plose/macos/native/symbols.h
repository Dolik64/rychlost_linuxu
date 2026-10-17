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
