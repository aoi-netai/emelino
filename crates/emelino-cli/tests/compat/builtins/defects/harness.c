
#include <assert.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf jump;static int defects;
int32_t should_not_run(void){abort();}
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char*message,uint16_t length){
  if(defects==0){assert(kind==EMELA_DEFECT_TODO);assert(length==19&&!memcmp(message,"not yet implemented",19));}
  else if(defects==1) {assert(kind==EMELA_DEFECT_PANIC);assert(length==6&&!memcmp(message,"halt42",6));}
  else {assert(kind==EMELA_DEFECT_ASSERT);assert(length>=17&&!memcmp(message,"assertion failed:",17));}
  defects++;longjmp(jump,1);
}
int main(void){
  if(!setjmp(jump)){(void)placeholder();abort();}
  if(!setjmp(jump)){(void)dynamic_panic(42);abort();}
  assert(checked(42)==42);
  if(!setjmp(jump)){(void)checked(0);abort();}
  assert(defects==3);return 0;
}
