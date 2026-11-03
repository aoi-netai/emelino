
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
#include "emela_embedded.h"
int main(void) { for(int round=0;round<1000;round++){
  EmelaString both = {.data = "aba", .length = 3};
  EmelaString middle = {.data = "bab", .length = 3};
  EmelaString empty = {.data = "", .length = 0};
  assert(legacy_has_a(both) && legacy_starts_a(both) && legacy_ends_a(both));
  assert(legacy_has_a(middle) && !legacy_starts_a(middle) && !legacy_ends_a(middle));
  assert(!legacy_has_a(empty) && !legacy_starts_a(empty) && !legacy_ends_a(empty));
  }return 0;
}
