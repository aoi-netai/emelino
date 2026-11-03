
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}

int main(void){for(int i=0;i<1000;i++){assert(legacy_work(0)==42&&test_heap_empty());assert(legacy_work(1)==3&&test_heap_empty());}return 0;}
