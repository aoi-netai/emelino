#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t old) { (void)old; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) {(void)k;(void)m;(void)n;abort();}
int main(void) { return 0; }
