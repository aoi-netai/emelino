
int test_children(void){int children=0;for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free&&b->kind==EMELA_CHILD_KIND)children++;p+=b->size;}return children;}
void test_cancel(EmelaFiber f){emela_fiber_get(f)->cancelled=1;}
