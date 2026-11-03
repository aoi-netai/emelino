#include "emela_embedded.h"
#include "emela_board.h"
#include <stdlib.h>
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  (void)kind; (void)message; (void)length; abort();
}
int main(void) { return answer() == 42 ? 0 : 1; }
