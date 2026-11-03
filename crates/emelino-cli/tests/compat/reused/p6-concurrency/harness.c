
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",(int)k,(int)n,m);abort();}
static void finish(EmelaFiber fiber,int32_t*result){while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));assert(*result==42);emela_fiber_release(fiber);assert(test_heap_empty());}
int main(void){for(int i=0;i<1000;i++){assert(p6_attempt()==42&&test_heap_empty());assert(p6_cause()&&test_heap_empty());assert(p6_raise()==42&&test_heap_empty());int32_t result=0;EmelaFiber fiber=p6_scope_start(&result);finish(fiber,&result);result=0;fiber=p6_unit_start(&result);finish(fiber,&result);result=0;fiber=p6_deferred_start(&result);finish(fiber,&result);result=0;fiber=p6_deferred_unit_start(&result);finish(fiber,&result);}return 0;}
