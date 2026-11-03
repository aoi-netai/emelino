
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

int main(void){for(int i=0;i<1000;i++){assert(legacy_main()==82);assert(test_heap_empty());}return 0;}
