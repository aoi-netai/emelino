
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int reads,logs;
int32_t read_once(void){reads++;return 7;}
void emela_board_debug(const char*s,uint16_t n){const char expected[]="Outer(key: K7, inner: Inner(text: \"x\"))";assert(n==sizeof(expected)-1&&!memcmp(s,expected,n));logs++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(debug_value()==7&&reads==1&&logs==1);return 0;}
