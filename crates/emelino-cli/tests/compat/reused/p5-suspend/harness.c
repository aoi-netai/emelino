
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask pending;
static int32_t value;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
void board_wait_start(EmelaTask task,int32_t n){pending=task;value=n;}
int main(void){for(int i=0;i<1000;i++){
 int32_t result=0;EmelaFiber fiber=source_suspend_start(&result);
 while(emela_scheduler_run(1)){}
 assert(!emela_fiber_done(fiber)&&result==0);
 assert(emela_task_complete_int(pending,value));
 assert(!emela_task_complete_int(pending,7));
 assert(!emela_fiber_done(fiber));
 while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));
 assert(result==42);emela_fiber_release(fiber);
 assert(!emela_task_complete_int(pending,8)&&test_heap_empty());
}return 0;}
