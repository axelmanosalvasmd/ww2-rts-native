/* The consumer supports ABI 2.1. Later additive exports do not authorize their adoption. */
#include "ww2_core.h"
#include <stdio.h>

_Static_assert(WW2_ABI_VERSION == 2, "consumer requires ABI 2");
_Static_assert(WW2_ABI_MINOR_VERSION >= 1, "consumer requires ABI 2.1 declarations");
_Static_assert(sizeof(ww2_view_unit) == 152, "legacy row layout changed");
_Static_assert(offsetof(ww2_view_unit, x) == 72, "legacy row offset changed");
_Static_assert(offsetof(ww2_view_unit, stars) == 148, "legacy row offset changed");

int main(void)
{
    uint32_t major = ww2_abi_version();
    if (major != 2) return 1; /* Do not call any additive symbol after a major mismatch. */
    uint32_t minor = ww2_abi_minor_version();
    if (minor < 1) return 2;
    /* Link every approved export with its exact signature. No simulation or author map is needed. */
    typedef int32_t (*query)(const char *, size_t, char *, size_t, size_t *);
    query calls[] = {ww2_authoring_validate_map, ww2_authoring_migrate_scenario,
        ww2_authoring_migrate_map, ww2_authoring_spawns_for, ww2_authoring_create_preview,
        ww2_authoring_find_path, ww2_authoring_house_kinds, ww2_authoring_play_match};
    for (size_t i = 0; i < sizeof calls / sizeof calls[0]; ++i) {
        size_t count = 99;
        if (calls[i](NULL, 0, NULL, 0, &count) != WW2_ERR_PARSE || count != 0) return 3;
    }
    size_t count = 99;
    if (ww2_view_placement_check(NULL, "{}", 2, NULL, 0, &count) != WW2_ERR_NULL_POINTER) return 4;
    printf("ABI guard: library %u.%u, header %u.%u, consumer requires 2.1; 9 query symbols linked\n",
        major, minor, WW2_ABI_VERSION, WW2_ABI_MINOR_VERSION);
    return 0;
}
