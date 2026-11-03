
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void emela_platform_start(void(*program)(void)){program();}
void emela_board_init(void){}
void emela_board_idle(void){}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,(int)n,m);abort();}


#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int main(void) {
  for (int i = 0; i < 1000; i++) {
    emela_function_type_{ty} f = legacy_formatter(INT32_MIN);
    EmelaString value = f.call(f.env);
    emela_embedded_release_function_{ty}(&f);
    assert(value.owner && value.length == 11 && memcmp(value.data, "-2147483648", 11) == 0);
    emela_embedded_release_string(&value);assert(test_heap_empty());
    f = legacy_capture(42);
    value = f.call(f.env);
    emela_embedded_release_function_{ty}(&f);
    assert(value.owner && value.length == 2 && memcmp(value.data, "42", 2) == 0);
    emela_embedded_release_string(&value);assert(test_heap_empty());
  }
}
