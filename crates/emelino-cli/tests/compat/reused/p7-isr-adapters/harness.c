
#include <assert.h>
#include <string.h>
#include "emela_embedded.h"
int32_t board_tick(int32_t value){return value;}
double board_float(double value){return value;}
int main(void){
 const uint64_t bits[]={0,UINT64_C(0x8000000000000000),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8000000000042),1,UINT64_C(0x7fefffffffffffff)};
 for(int i=0;i<1000;i++){
  assert(TIM2_IRQHandler(INT32_MIN)==INT32_MIN);
  assert(TIM2_IRQHandler(INT32_MAX)==INT32_MAX);
  for(unsigned j=0;j<sizeof(bits)/sizeof(*bits);j++){
   double value;memcpy(&value,&bits[j],sizeof(value));value=float_bits(value);
   uint64_t result;memcpy(&result,&value,sizeof(result));assert(result==bits[j]);
  }
 }
 return 0;
}
