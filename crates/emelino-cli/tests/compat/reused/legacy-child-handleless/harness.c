
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_children(void);int test_heap_empty(void);void test_cancel(EmelaFiber);
static EmelaTask tasks[3];static int started;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
void board_wait_start(EmelaTask task,int32_t n){assert(n>=1&&n<=3);tasks[n-1]=task;started++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"%d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++){EmelaString result={0};started=0;EmelaFiber f=work_start(42,i%3?&result:NULL);while(emela_scheduler_run(1)){}assert(started==3&&!emela_fiber_done(f)&&test_children()==1);if(i%2){test_cancel(f);while(emela_scheduler_run(1)){}}assert(!emela_fiber_done(f)&&test_children()==1);assert(emela_task_complete_int(tasks[2],0));while(!emela_fiber_done(f))assert(emela_scheduler_run(1));emela_fiber_release(f);assert(!emela_task_complete_int(tasks[0],0)&&!emela_task_complete_int(tasks[1],0)&&test_children()==0);if(i%3&&i%2==0){assert(result.owner&&result.length==2&&!memcmp(result.data,"42",2));emela_embedded_release_string(&result);}else{assert(result.data==NULL&&result.owner==NULL&&result.length==0);}assert(test_heap_empty());}return 0;}
