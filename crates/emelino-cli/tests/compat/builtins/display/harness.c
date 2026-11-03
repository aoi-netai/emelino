
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int logs;
void emela_board_debug(const char*s,uint16_t n) {
  const char expected[]="Item(value: \"x\\n\\\"\")";
  assert(n==sizeof(expected)-1 && !memcmp(s,expected,n));logs++;
}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void) {
  char output[256];EmelaString input={"x\n\"",3,NULL};
  EmelaString value=display(input,output,sizeof output);
  const char expected[]="(Item(value: \"x\\n\\\"\"), \"x\\n\\\"\")";
  assert(value.length==sizeof(expected)-1 && !memcmp(value.data,expected,value.length));
  assert(logs==1);
  value=nested(input,output,sizeof output);
  const char quoted[]="\"x\\n\\\"\"";
  assert(value.length==sizeof(quoted)-1 && !memcmp(value.data,quoted,value.length));
  return 0;
}
