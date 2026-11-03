
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}

int main(void){for(int i=0;i<1000;i++){char buffer[128];EmelaString text=legacy_show(buffer,sizeof buffer);static const char expected[]="error: Boom(value: Box([42]))";assert(text.length==sizeof expected-1&&!memcmp(text.data,expected,sizeof expected-1));assert(test_heap_empty());}return 0;}
