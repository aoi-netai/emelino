
#include <assert.h>
#include "emela_embedded.h"
int main(void){for(int i=0;i<1000;i++){assert(read_static()==42);assert(bytes_size((EmelaBytes){.length=70000})==70000);assert(compare((EmelaString){.data="a\0b",.length=3},(EmelaString){.data="a\0c",.length=3}));assert(!compare((EmelaString){.data="z",.length=1},(EmelaString){.data="a",.length=1}));}return 0;}
