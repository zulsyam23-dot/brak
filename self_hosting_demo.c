#include <stdint.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

typedef struct Lexer {
    int32_t pos;
    int32_t len;
} Lexer;



int64_t next_char(int64_t r0)
{
    int64_t r1, r2, r3, r4, r5, r6, r7, r8, r9;

    block_0:
#line 7 "samples/self_hosting_demo.brk"
        r1 = ((struct _GenericStruct*)(uintptr_t)r0)->pos;
        r2 = ((struct _GenericStruct*)(uintptr_t)r0)->len;
        r3 = (r1 >= r2) ? 1 : 0;
        if (r3) goto block_1; else goto block_2;

    block_1:
#line 8 "samples/self_hosting_demo.brk"
        r4 = 0;
        return r4;

    block_2:
#line 10 "samples/self_hosting_demo.brk"
        r5 = ((struct _GenericStruct*)(uintptr_t)r0)->pos;
        r6 = 1;
        r7 = r5 + r6;
        ((struct _GenericStruct*)(uintptr_t)r0)->pos = r7;
#line 11 "samples/self_hosting_demo.brk"
        r9 = 1;
        return r9;
}

int64_t main(void)
{
    int64_t r1, r2, r3, r0, r5, r4, r8, r9, r10, r6, r7;

    block_0:
#line 16 "samples/self_hosting_demo.brk"
        r1 = 0;
#line 17 "samples/self_hosting_demo.brk"
        r2 = 10;
#line 15 "samples/self_hosting_demo.brk"
        r3 = (int64_t)(uintptr_t)calloc(1, sizeof(struct Lexer));
        ((struct Lexer*)(uintptr_t)r3)->pos = r1;
        ((struct Lexer*)(uintptr_t)r3)->len = r2;
        r0 = r3;
#line 20 "samples/self_hosting_demo.brk"
        r5 = 0;
        r4 = r5;
#line 21 "samples/self_hosting_demo.brk"
        goto block_1;

    block_1:
        r8 = next_char(r0);
        r9 = 0;
        r10 = (r8 != r9) ? 1 : 0;
        if (r10) goto block_2; else goto block_3;

    block_2:
#line 22 "samples/self_hosting_demo.brk"
        r6 = 1;
        r7 = r4 + r6;
        r4 = r7;
#line 21 "samples/self_hosting_demo.brk"
        goto block_1;

    block_3:
#line 25 "samples/self_hosting_demo.brk"
        return r4;
}

