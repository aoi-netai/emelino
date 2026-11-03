#include "emela_embedded.h"
#include "emela_board.h"
#include <stdlib.h>
static int calls;
void emela_board_debug(const char *bytes,uint16_t length) { (void)bytes;(void)length;calls++; }
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char *message,uint16_t length) { (void)kind;(void)message;(void)length;abort(); }
int main(void) {
  if (probe()!=0 || calls!=2) return 1;
  if (probe()!=0 || calls!=2) return 2;
  return 0;
}
